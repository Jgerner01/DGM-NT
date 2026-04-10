using System;
using System.IO;
using System.Windows;
using System.Windows.Controls;
using System.Text.RegularExpressions;

namespace DGM_NT.Views;

/// <summary>
/// Dialog für neues Projekt
/// </summary>
public class ProjektNeuDialog : Window
{
    private TextBox _txtName = null!;
    private TextBox _txtPfad = null!;
    private TextBox _txtOrt = null!;
    private TextBox _txtBearbeiter = null!;
    private Label _lblFehler = null!;
    
    public string ProjektName { get; private set; } = "";
    public string ProjektPfad { get; private set; } = "";
    public string Ort { get; private set; } = "";
    public string Bearbeiter { get; private set; } = "";
    
    // Standard-Basisverzeichnis
    private string _basisPfad = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.MyDocuments), "DGM-NT-Projekte");
    
    public ProjektNeuDialog()
    {
        Title = "Neues Projekt erstellen";
        Width = 550;
        Height = 380;
        WindowStartupLocation = WindowStartupLocation.CenterOwner;
        ResizeMode = ResizeMode.NoResize;
        Background = new System.Windows.Media.SolidColorBrush(System.Windows.Media.Color.FromRgb(245, 245, 245));
        
        var grid = new Grid { Margin = new Thickness(20) };
        grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto }); // Titel
        grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto }); // Name
        grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto }); // Pfad
        grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto }); // Ort
        grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto }); // Bearbeiter
        grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto }); // Fehler
        grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto }); // Buttons
        
        // Titel
        var titel = new TextBlock
        {
            Text = "Neues DGM-Projekt anlegen",
            FontSize = 18,
            FontWeight = System.Windows.FontWeights.Bold,
            Margin = new Thickness(0, 0, 0, 15)
        };
        Grid.SetRow(titel, 0);
        grid.Children.Add(titel);
        
        // Projektname
        var lblName = new Label { Content = "_Projektname:", Margin = new Thickness(0, 0, 0, 2) };
        Grid.SetRow(lblName, 1);
        grid.Children.Add(lblName);
        
        var namePanel = new StackPanel { Margin = new Thickness(0, 0, 0, 10) };
        _txtName = new TextBox 
        { 
            FontSize = 14, 
            Padding = new Thickness(5),
            ToolTip = "Name des Projekts (z.B. Baugebiet-Nord)"
        };
        _txtName.TextChanged += (s, e) => Validate();
        namePanel.Children.Add(new Label { Content = "Projektname:", FontWeight = System.Windows.FontWeights.SemiBold, Padding = new Thickness(0) });
        namePanel.Children.Add(_txtName);
        namePanel.Children.Add(new TextBlock 
        { 
            Text = "Es wird ein neues Verzeichnis mit diesem Namen erstellt.",
            FontSize = 11,
            Foreground = System.Windows.Media.Brushes.Gray,
            Margin = new Thickness(0, 3, 0, 0)
        });
        Grid.SetRow(namePanel, 1);
        grid.Children.Add(namePanel);
        
        // Verzeichnispfad
        var pfadPanel = new StackPanel { Margin = new Thickness(0, 5, 0, 10) };
        pfadPanel.Children.Add(new Label { Content = "Projektverzeichnis:", FontWeight = System.Windows.FontWeights.SemiBold, Padding = new Thickness(0) });
        
        var pfadRow = new Grid();
        pfadRow.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        pfadRow.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        
        _txtPfad = new TextBox 
        { 
            FontSize = 12, 
            Padding = new Thickness(5),
            IsReadOnly = true,
            Background = System.Windows.Media.Brushes.WhiteSmoke,
            Margin = new Thickness(0, 0, 5, 0)
        };
        Grid.SetColumn(_txtPfad, 0);
        pfadRow.Children.Add(_txtPfad);
        
        var btnDurchsuchen = new Button { Content = "Durchsuchen...", Padding = new Thickness(10, 5, 10, 5) };
        btnDurchsuchen.Click += BtnDurchsuchen_Click;
        Grid.SetColumn(btnDurchsuchen, 1);
        pfadRow.Children.Add(btnDurchsuchen);
        
        pfadPanel.Children.Add(pfadRow);
        pfadPanel.Children.Add(new TextBlock 
        { 
            Text = "Basisverzeichnis für alle DGM-NT-Projekte",
            FontSize = 11,
            Foreground = System.Windows.Media.Brushes.Gray,
            Margin = new Thickness(0, 3, 0, 0)
        });
        Grid.SetRow(pfadPanel, 2);
        grid.Children.Add(pfadPanel);
        
        // Ort
        var ortPanel = new StackPanel { Margin = new Thickness(0, 5, 0, 10) };
        ortPanel.Children.Add(new Label { Content = "Ort / Beschreibung:", FontWeight = System.Windows.FontWeights.SemiBold, Padding = new Thickness(0) });
        _txtOrt = new TextBox { FontSize = 12, Padding = new Thickness(5) };
        ortPanel.Children.Add(_txtOrt);
        Grid.SetRow(ortPanel, 3);
        grid.Children.Add(ortPanel);
        
        // Bearbeiter
        var bearbPanel = new StackPanel { Margin = new Thickness(0, 5, 0, 10) };
        bearbPanel.Children.Add(new Label { Content = "Bearbeiter:", FontWeight = System.Windows.FontWeights.SemiBold, Padding = new Thickness(0) });
        _txtBearbeiter = new TextBox { FontSize = 12, Padding = new Thickness(5) };
        bearbPanel.Children.Add(_txtBearbeiter);
        Grid.SetRow(bearbPanel, 4);
        grid.Children.Add(bearbPanel);
        
        // Fehleranzeige
        _lblFehler = new Label 
        { 
            Foreground = System.Windows.Media.Brushes.Red, 
            Visibility = Visibility.Collapsed,
            Margin = new Thickness(0, 5, 0, 5)
        };
        Grid.SetRow(_lblFehler, 5);
        grid.Children.Add(_lblFehler);
        
        // Buttons
        var btnPanel = new StackPanel { Orientation = Orientation.Horizontal, HorizontalAlignment = HorizontalAlignment.Right, Margin = new Thickness(0, 10, 0, 0) };
        
        var btnOk = new Button 
        { 
            Content = "Projekt erstellen", 
            Padding = new Thickness(20, 8, 20, 8),
            Margin = new Thickness(0, 0, 10, 0),
            IsDefault = true
        };
        btnOk.Click += BtnOk_Click;
        btnPanel.Children.Add(btnOk);
        
        var btnAbbruch = new Button { Content = "Abbruch", Padding = new Thickness(15, 8, 15, 8), IsCancel = true };
        btnAbbruch.Click += (s, e) => DialogResult = false;
        btnPanel.Children.Add(btnAbbruch);
        
        Grid.SetRow(btnPanel, 6);
        grid.Children.Add(btnPanel);
        
        Content = grid;
        
        // Initial-Pfad setzen
        _txtPfad.Text = _basisPfad;
        
        // Validieren
        Validate();
        _txtName.Focus();
    }
    
    private void BtnDurchsuchen_Click(object sender, RoutedEventArgs e)
    {
        var dialog = new Microsoft.Win32.OpenFolderDialog();
        dialog.Title = "Basisverzeichnis für Projekte auswählen";
        
        if (!string.IsNullOrEmpty(_basisPfad) && Directory.Exists(_basisPfad))
            dialog.InitialDirectory = _basisPfad;
        
        if (dialog.ShowDialog() == true)
        {
            _basisPfad = dialog.FolderName;
            _txtPfad.Text = _basisPfad;
            Validate();
        }
    }
    
    private void Validate()
    {
        string name = _txtName.Text.Trim();
        bool ok = true;
        string fehler = "";
        
        if (string.IsNullOrEmpty(name))
        {
            ok = false;
            fehler = "Bitte einen Projektnamen eingeben.";
        }
        else if (name.Length > 50)
        {
            ok = false;
            fehler = "Projektname maximal 50 Zeichen.";
        }
        else if (!Regex.IsMatch(name, @"^[a-zA-Z0-9äöüÄÖÜß\s\-_]+$"))
        {
            ok = false;
            fehler = "Projektname enthält ungültige Zeichen. Nur Buchstaben, Zahlen, - und _ erlaubt.";
        }
        
        if (ok)
        {
            string neuerPfad = Path.Combine(_txtPfad.Text.Trim(), name);
            if (Directory.Exists(neuerPfad))
            {
                ok = false;
                fehler = $"Verzeichnis existiert bereits:\n{neuerPfad}";
            }
        }
        
        if (ok)
        {
            _lblFehler.Visibility = Visibility.Collapsed;
            _lblFehler.Content = "";
        }
        else
        {
            _lblFehler.Visibility = Visibility.Visible;
            _lblFehler.Content = fehler;
        }
    }
    
    private void BtnOk_Click(object sender, RoutedEventArgs e)
    {
        string name = _txtName.Text.Trim();
        string pfad = _txtPfad.Text.Trim();
        
        if (string.IsNullOrEmpty(name) || string.IsNullOrEmpty(pfad))
            return;
        
        // Verzeichnis erstellen
        string projektVerzeichnis = Path.Combine(pfad, name);
        
        try
        {
            Directory.CreateDirectory(projektVerzeichnis);
            
            ProjektName = name;
            ProjektPfad = projektVerzeichnis;
            Ort = _txtOrt.Text.Trim();
            Bearbeiter = _txtBearbeiter.Text.Trim();
            
            DialogResult = true;
            Close();
        }
        catch (Exception ex)
        {
            _lblFehler.Visibility = Visibility.Visible;
            _lblFehler.Content = $"Fehler beim Erstellen des Verzeichnisses:\n{ex.Message}";
        }
    }
}
