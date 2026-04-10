using System;
using System.IO;
using System.Windows;
using System.Windows.Controls;
using DGM_NT.Services;

namespace DGM_NT.Views;

/// <summary>
/// Dialog zur Parametrisierung der Dreiecksvermaschung
/// Nach dem Delphi-Original (Form5 / UNIT5.PAS / UVAR.PAS)
/// </summary>
public class TinParameterDialog : Window
{
    private TextBox _txtSuchradius = null!;
    private TextBox _txtToleranz = null!;
    private ComboBox _cmbDichte = null!;
    private TextBox _txtMinFlaeche = null!;
    private CheckBox _chkDegenerierte = null!;
    private TextBlock _vorschauText = null!;
    private int _anzPunkte = 0;

    public double Suchradius { get; private set; } = 0;       // sm: Suchradius für Vermaschung
    public double Toleranz { get; private set; } = 0.0001;    // tol: Toleranz für Umkreis-Test
    public int Dichte { get; private set; } = 1;              // dichte: Dreiecksunterteilung
    public double MinFlaeche { get; private set; } = 1e-10;   // Minimale Dreiecksfläche
    public bool DegenerierteEntfernen { get; private set; } = true;

    public TinParameterDialog(int anzPunkte)
    {
        _anzPunkte = anzPunkte;
        var appParams = AppParameter.Laden();

        Title = "Vermaschung - Parameter";
        Width = 440;
        Height = 500;
        WindowStartupLocation = WindowStartupLocation.CenterOwner;
        ResizeMode = ResizeMode.NoResize;
        Background = new System.Windows.Media.SolidColorBrush(System.Windows.Media.Color.FromRgb(245, 245, 245));

        var scrollViewer = new ScrollViewer 
        { 
            VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
            HorizontalScrollBarVisibility = ScrollBarVisibility.Disabled
        };

        var mainGrid = new Grid { Margin = new Thickness(20) };
        int row = 0;
        mainGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });  // Titel
        mainGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });  // Info
        mainGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });  // Suchradius
        mainGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });  // Toleranz
        mainGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });  // Dichte
        mainGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });  // Min Fläche
        mainGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });  // Degenerierte
        mainGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });  // Vorschau
        mainGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });  // Buttons

        // Titel
        var titel = new TextBlock 
        { 
            Text = "🔺 Vermaschung - Parameter", 
            FontSize = 18, 
            FontWeight = FontWeights.Bold, 
            Margin = new Thickness(0, 0, 0, 5) 
        };
        Grid.SetRow(titel, row++);
        mainGrid.Children.Add(titel);

        // Info
        var infoBorder = new Border 
        { 
            Background = System.Windows.Media.Brushes.LightBlue, 
            BorderBrush = System.Windows.Media.Brushes.SteelBlue, 
            BorderThickness = new Thickness(1), 
            CornerRadius = new CornerRadius(4), 
            Padding = new Thickness(10), 
            Margin = new Thickness(0, 5, 0, 10) 
        };
        var infoPanel = new StackPanel();
        infoPanel.Children.Add(new TextBlock { Text = "Eingabedaten:", FontWeight = FontWeights.SemiBold, FontSize = 12 });
        infoPanel.Children.Add(new TextBlock { Text = $"Anzahl Punkte: {anzPunkte}", FontSize = 12, Margin = new Thickness(0, 2, 0, 0) });
        infoPanel.Children.Add(new TextBlock { Text = $"Erwartete Dreiecke: ~{anzPunkte * 2}", FontSize = 12 });
        infoBorder.Child = infoPanel;
        Grid.SetRow(infoBorder, row++);
        mainGrid.Children.Add(infoBorder);

        // Suchradius (sm)
        var srPanel = new StackPanel { Margin = new Thickness(0, 0, 0, 8) };
        srPanel.Children.Add(new Label { Content = "_Suchradius (sm): 0 = unbegrenzt", FontWeight = FontWeights.SemiBold, Padding = new Thickness(0) });
        srPanel.Children.Add(new TextBlock { Text = "Punkte die weiter als dieser Wert entfernt sind,\nwerden nicht vermascht (Delphi: sm)", FontSize = 10, Foreground = System.Windows.Media.Brushes.Gray, Margin = new Thickness(0, 0, 0, 3) });
        _txtSuchradius = new TextBox { FontSize = 12, Padding = new Thickness(5), Text = appParams.TinSuchradius.ToString("F1") };
        srPanel.Children.Add(_txtSuchradius);
        Grid.SetRow(srPanel, row++);
        mainGrid.Children.Add(srPanel);

        // Toleranz (tol)
        var tolPanel = new StackPanel { Margin = new Thickness(0, 0, 0, 8) };
        tolPanel.Children.Add(new Label { Content = "_Toleranz (tol) für Umkreis-Test:", FontWeight = FontWeights.SemiBold, Padding = new Thickness(0) });
        tolPanel.Children.Add(new TextBlock { Text = "Delphi-Default: 0.0001", FontSize = 10, Foreground = System.Windows.Media.Brushes.Gray, Margin = new Thickness(0, 0, 0, 3) });
        _txtToleranz = new TextBox { FontSize = 12, Padding = new Thickness(5), Text = appParams.TinToleranz.ToString("G6") };
        tolPanel.Children.Add(_txtToleranz);
        Grid.SetRow(tolPanel, row++);
        mainGrid.Children.Add(tolPanel);

        // Dichte
        var dPanel = new StackPanel { Margin = new Thickness(0, 0, 0, 8) };
        dPanel.Children.Add(new Label { Content = "_Dichte (Unterteilungsgrad):", FontWeight = FontWeights.SemiBold, Padding = new Thickness(0) });
        dPanel.Children.Add(new TextBlock { Text = "Delphi: dichte (1..15). Für Höhenlinien-Interpolation.", FontSize = 10, Foreground = System.Windows.Media.Brushes.Gray, Margin = new Thickness(0, 0, 0, 3) });
        _cmbDichte = new ComboBox { FontSize = 12, Padding = new Thickness(5) };
        for (int i = 1; i <= 15; i++)
        {
            _cmbDichte.Items.Add(new ComboBoxItem { Content = i.ToString(), Tag = i });
            if (i == appParams.TinDichte) _cmbDichte.SelectedIndex = i - 1;
        }
        if (_cmbDichte.SelectedIndex < 0) _cmbDichte.SelectedIndex = 0;
        dPanel.Children.Add(_cmbDichte);
        Grid.SetRow(dPanel, row++);
        mainGrid.Children.Add(dPanel);

        // Minimale Fläche
        var mfPanel = new StackPanel { Margin = new Thickness(0, 0, 0, 8) };
        mfPanel.Children.Add(new Label { Content = "Minimale Dreiecksfläche (Entfernung degenerierter Dreiecke):", FontWeight = FontWeights.SemiBold, Padding = new Thickness(0) });
        _txtMinFlaeche = new TextBox { FontSize = 12, Padding = new Thickness(5), Text = appParams.TinMinFlaeche.ToString("E2") };
        mfPanel.Children.Add(_txtMinFlaeche);
        Grid.SetRow(mfPanel, row++);
        mainGrid.Children.Add(mfPanel);

        // Degenerierte entfernen
        _chkDegenerierte = new CheckBox 
        { 
            Content = "Degenerierte Dreiecke automatisch entfernen (Fläche ≈ 0)", 
            FontSize = 12, 
            Margin = new Thickness(0, 0, 0, 8), 
            IsChecked = appParams.TinDegenerierteEntfernen 
        };
        Grid.SetRow(_chkDegenerierte, row++);
        mainGrid.Children.Add(_chkDegenerierte);

        // Vorschau
        var vorschauBorder = new Border 
        { 
            Background = System.Windows.Media.Brushes.LightYellow, 
            BorderBrush = System.Windows.Media.Brushes.Goldenrod, 
            BorderThickness = new Thickness(1), 
            CornerRadius = new CornerRadius(4), 
            Padding = new Thickness(10), 
            Margin = new Thickness(0, 5, 0, 10) 
        };
        var vorschauPanel = new StackPanel();
        var vorschauLabel = new TextBlock { Text = "📊 Vorschau:", FontWeight = FontWeights.SemiBold, FontSize = 12 };
        _vorschauText = new TextBlock { Text = "", FontSize = 11, TextWrapping = TextWrapping.Wrap };
        vorschauPanel.Children.Add(vorschauLabel);
        vorschauPanel.Children.Add(_vorschauText);
        vorschauBorder.Child = vorschauPanel;
        Grid.SetRow(vorschauBorder, row++);
        mainGrid.Children.Add(vorschauBorder);

        VorschauAktualisieren();

        // Buttons
        var btnPanel = new StackPanel { Orientation = Orientation.Horizontal, HorizontalAlignment = HorizontalAlignment.Right, Margin = new Thickness(0, 10, 0, 0) };
        var btnOk = new Button 
        { 
            Content = "✓ OK – Parameter speichern & Vermaschen", 
            Padding = new Thickness(20, 10, 20, 10), 
            FontSize = 13, 
            FontWeight = FontWeights.SemiBold, 
            IsDefault = true 
        };
        btnOk.Click += BtnOk_Click;
        btnPanel.Children.Add(btnOk);
        var btnAbbruch = new Button { Content = "Abbruch", Padding = new Thickness(15, 8, 15, 8), IsCancel = true };
        btnAbbruch.Click += (s, e) => DialogResult = false;
        btnPanel.Children.Add(btnAbbruch);
        Grid.SetRow(btnPanel, row++);
        mainGrid.Children.Add(btnPanel);

        scrollViewer.Content = mainGrid;
        Content = scrollViewer;
    }

    private void VorschauAktualisieren()
    {
        if (double.TryParse(_txtSuchradius.Text, out double sr))
        {
            _vorschauText.Text = $"Suchradius: {(sr == 0 ? "Unbegrenzt" : sr.ToString("F1") + " m")}\n" +
                                $"Erwartete Dreiecke: ~{_anzPunkte * 2}\n" +
                                $"Toleranz: {_txtToleranz.Text}\n" +
                                $"Dichte: {_cmbDichte.Text}";
        }
    }

    private void BtnOk_Click(object sender, RoutedEventArgs e)
    {
        // Werte lesen
        double.TryParse(_txtSuchradius.Text, out double sr);
        double.TryParse(_txtToleranz.Text, out double tol);
        if (_cmbDichte.SelectedItem is ComboBoxItem dItem && dItem.Tag is int d) Dichte = d;
        double.TryParse(_txtMinFlaeche.Text, out double minF);
        
        Suchradius = sr;
        Toleranz = tol > 0 ? tol : 0.0001;
        MinFlaeche = minF > 0 ? minF : 1e-10;
        DegenerierteEntfernen = _chkDegenerierte.IsChecked == true;

        // Speichern
        var appParams = AppParameter.Laden();
        appParams.TinSuchradius = Suchradius;
        appParams.TinToleranz = Toleranz;
        appParams.TinDichte = Dichte;
        appParams.TinMinFlaeche = MinFlaeche;
        appParams.TinDegenerierteEntfernen = DegenerierteEntfernen;
        appParams.Speichern();

        DialogResult = true;
        Close();
    }
}
