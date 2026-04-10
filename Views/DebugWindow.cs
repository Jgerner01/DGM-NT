using System;
using System.IO;
using System.Windows;
using System.Windows.Controls;
using System.Windows.Media;
using System.Diagnostics;

namespace DGM_NT.Views;

/// <summary>
/// Debug-Fenster das beim Start alle Initialisierungsschritte anzeigt
/// </summary>
public class DebugWindow : Window
{
    private TextBox _logBox = null!;
    private string _logText = "";
    private int _schritt = 0;
    
    public DebugWindow()
    {
        Title = "DGM-NT Debug-Information";
        Width = 900;
        Height = 650;
        WindowStartupLocation = WindowStartupLocation.CenterScreen;
        Background = new SolidColorBrush(Color.FromRgb(30, 30, 30));
        ResizeMode = ResizeMode.CanResizeWithGrip;
        
        // Grid aufbauen
        var grid = new Grid();
        grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        grid.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        
        // Titel
        var titel = new TextBlock
        {
            Text = "DGM-NT Debug-Information",
            Foreground = Brushes.White,
            FontSize = 18,
            FontWeight = FontWeights.Bold,
            Margin = new Thickness(15, 10, 15, 5)
        };
        Grid.SetRow(titel, 0);
        grid.Children.Add(titel);
        
        // Log-Box
        _logBox = new TextBox
        {
            Background = new SolidColorBrush(Color.FromRgb(15, 15, 15)),
            Foreground = Brushes.Lime,
            FontFamily = new FontFamily("Consolas"),
            FontSize = 13,
            IsReadOnly = true,
            VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
            HorizontalScrollBarVisibility = ScrollBarVisibility.Auto,
            Margin = new Thickness(10),
            AcceptsReturn = true,
            TextWrapping = TextWrapping.Wrap
        };
        Grid.SetRow(_logBox, 1);
        grid.Children.Add(_logBox);
        
        // Status-Leiste unten
        var statusPanel = new StackPanel { Orientation = Orientation.Horizontal, Margin = new Thickness(10, 5, 10, 10) };
        
        var statusLabel = new TextBlock
        {
            Text = "Status:",
            Foreground = Brushes.White,
            Margin = new Thickness(0, 0, 10, 0)
        };
        _statusText = new TextBlock
        {
            Text = "Initialisierung...",
            Foreground = Brushes.Yellow,
            FontWeight = FontWeights.Bold
        };
        
        statusPanel.Children.Add(statusLabel);
        statusPanel.Children.Add(_statusText);
        Grid.SetRow(statusPanel, 2);
        grid.Children.Add(statusPanel);
        
        Content = grid;
    }
    
    private TextBlock _statusText = null!;
    
    public void AddLog(string text)
    {
        _schritt++;
        string zeile = $"[{_schritt:D2}] {DateTime.Now:HH:mm:ss.fff} | {text}";
        _logText += zeile + Environment.NewLine;
        _logBox.Text = _logText;
        _logBox.ScrollToEnd();
        
        // Auch in Datei schreiben
        try
        {
            File.AppendAllText(
                Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "dgm-nt-debug.log"),
                zeile + Environment.NewLine);
        }
        catch { }
    }
    
    public void SetStatus(string status, string farbe = "Yellow")
    {
        _statusText.Text = status;
        _statusText.Foreground = farbe switch
        {
            "Green" => Brushes.Lime,
            "Red" => Brushes.Red,
            "Yellow" => Brushes.Yellow,
            "Orange" => Brushes.Orange,
            _ => Brushes.White
        };
    }
}
